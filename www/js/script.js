$(document).ready(function() {
	function drawMe() {
		navigator.geolocation.watchPosition(function(position) {
			var latitude = position.coords.latitude;
			var longitude = position.coords.longitude;
			var gLatLng = new google.maps.LatLng(latitude, longitude);
			var nameLatLng = new google.maps.LatLng(latitude - 0.00004, longitude);
			var mapOptions = {
				center: gLatLng,
				zoom: 18,
				mapTypeId: google.maps.MapTypeId.ROADMAP,
				draggable: false,
				zoomControl: false,
				scrollwheel: false,
				disableDoubleClickZoom: true,
				disableDefaultUI: true,
			};
			var map = new google.maps.Map(document.getElementById("map-canvas"),mapOptions);
			var yourLocation = {
				path: google.maps.SymbolPath.CIRCLE,
				strokeColor: "#4c8f2a",
				strokeWeight: 2,
				fillColor: "#a2e346",
				fillOpacity: 1,
				scale: 6,
			};
			var drawCircle = new google.maps.Marker({
				position: gLatLng,
				icon: yourLocation,
				map: map,
			});
			var mapUserName = new MapLabel({
				text: 'Anonymous User',
				position: nameLatLng,
				map: map,
				fontSize: 14,
				fontColor: "#ffffff",
				strokeWeight: 4,
				strokeColor: "4c8f2a",
				fontFamily: "Avenir",
			});
			var changeUserName = document.getElementById('change-username');
			google.maps.event.addDomListener(changeUserName, 'click', function() {
				mapUserName.set('text', document.getElementById('userName').value);
			});
		}, function(error) {
			alert('Error occurred. Error code: ' + error.code);
		});
	};
	drawMe();

	function resizeText() {
		var windowHeight = $(window).height();
		var windowWidth = $(window).width();
		$("footer").css({
			'padding-top': windowHeight * 0.15 * 0.1 + 'px',
		});
	};
	resizeText();

	$(window).resize(function(){
		drawMe();
		resizeText();
	});
});